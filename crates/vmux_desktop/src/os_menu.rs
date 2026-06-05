use bevy::ecs::message::Messages;
use bevy::prelude::*;
use bevy::window::WindowCloseRequested;
use muda::{Menu, MenuEvent};
use parking_lot::Mutex;
use std::sync::LazyLock;
use vmux_command::{
    AppCommand, BrowserCommand, LayoutCommand, StackCommand, WriteAppCommands,
    build_native_root_menu, open::OpenCommand,
};
use vmux_setting::AppSettings;
use vmux_terminal as terminal;
use vmux_terminal::{PtyExited, Terminal};

/// Resource: window entity awaiting quit confirmation dialog.
#[derive(Resource, Default)]
pub(crate) struct PendingWindowClose {
    pub window: Option<Entity>,
}

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

static PENDING_MENU_EVENTS: LazyLock<Mutex<Vec<String>>> = LazyLock::new(|| Mutex::new(Vec::new()));
const WINDOW_CLOSE_SUPPRESSION_WINDOW: std::time::Duration = std::time::Duration::from_millis(300);
const NATIVE_PAGE_OPEN_CLOSE_SUPPRESSION_WINDOW: std::time::Duration =
    std::time::Duration::from_millis(1500);

#[allow(dead_code)]
struct OsMenuResource(Menu);

pub struct OsMenuPlugin;

impl Plugin for OsMenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PendingWindowClose>()
            .init_resource::<LastMenuCommandAt>()
            .init_resource::<LastStackCloseAt>()
            .init_resource::<LastNativePageOpenAt>()
            .add_systems(Startup, setup)
            .add_systems(
                Update,
                (
                    forward_menu_events.in_set(WriteAppCommands),
                    remember_stack_close_commands.after(WriteAppCommands),
                    remember_native_page_open_commands.after(WriteAppCommands),
                    close_with_confirmation
                        .after(remember_stack_close_commands)
                        .after(remember_native_page_open_commands),
                    process_pending_window_close,
                ),
            );
    }
}

fn setup(world: &mut World) {
    let mut menu = Menu::new();
    build_native_root_menu(&mut menu).unwrap();

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

    world.insert_non_send(OsMenuResource(menu));
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

/// Replacement for bevy's `close_when_requested` that shows a confirmation
/// dialog when terminals are still running. Defers the dialog to the
/// exclusive `show_pending_close_dialogs` system to avoid deadlocks.
fn close_with_confirmation(
    mut closed: MessageReader<WindowCloseRequested>,
    mut windows: Query<&mut Window>,
    settings: Res<AppSettings>,
    live_terminals: Query<(), (With<Terminal>, Without<PtyExited>)>,
    mut pending: ResMut<PendingWindowClose>,
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
        let should_confirm = terminal::should_confirm_close(&settings);
        let live_terminal_count = live_terminals.iter().count();
        info!(
            target: "vmux_desktop::window_close",
            window = ?event.window,
            should_confirm,
            live_terminal_count,
            "handling WindowCloseRequested"
        );
        if should_confirm && live_terminal_count > 0 {
            pending.window = Some(event.window);
        } else if let Ok(mut window) = windows.get_mut(event.window) {
            window.visible = false;
        }
    }
}

/// Exclusive system: processes pending window close confirmation by showing
/// a native dialog on the main thread.
fn process_pending_window_close(world: &mut World) {
    let window = world.resource::<PendingWindowClose>().window;
    let Some(window) = window else {
        return;
    };

    world.resource_mut::<PendingWindowClose>().window = None;

    let mut query = world.query_filtered::<(), (With<Terminal>, Without<PtyExited>)>();
    let count = query.iter(world).count();

    if (count == 0 || terminal::confirm_quit_dialog(count))
        && let Ok(mut entity_mut) = world.get_entity_mut(window)
        && let Some(mut win) = entity_mut.get_mut::<Window>()
    {
        win.visible = false;
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
    use vmux_setting::{AgentSettings, BrowserSettings, ShortcutSettings};

    fn test_settings() -> AppSettings {
        AppSettings {
            browser: BrowserSettings {
                startup_url: "about:blank".to_string(),
            },
            layout: LayoutSettings {
                radius: 0.0,
                window: WindowSettings {
                    padding: 0.0,
                    padding_top: None,
                    padding_right: None,
                    padding_bottom: None,
                    padding_left: None,
                },
                pane: PaneSettings { gap: 0.0 },
                side_sheet: SideSheetSettings::default(),
                focus_ring: FocusRingSettings::default(),
            },
            shortcuts: ShortcutSettings::default(),
            terminal: None,
            auto_update: false,
            agent: AgentSettings::default(),
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
}
