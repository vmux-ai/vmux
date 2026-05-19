use crate::command::{AppCommand, WriteAppCommands, build_native_root_menu};
use bevy::ecs::message::Messages;
use bevy::prelude::*;
use bevy::window::WindowCloseRequested;
use muda::{Menu, MenuEvent};
use parking_lot::Mutex;
use std::sync::LazyLock;
use vmux_settings::AppSettings;
use vmux_terminal as terminal;
use vmux_terminal::{PtyExited, Terminal};

/// Resource: window entity awaiting quit confirmation dialog.
#[derive(Resource, Default)]
pub(crate) struct PendingWindowClose {
    pub window: Option<Entity>,
}

static PENDING_MENU_EVENTS: LazyLock<Mutex<Vec<String>>> = LazyLock::new(|| Mutex::new(Vec::new()));

#[allow(dead_code)]
struct OsMenuResource(Menu);

pub struct OsMenuPlugin;

impl Plugin for OsMenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PendingWindowClose>()
            .add_systems(Startup, setup)
            .add_systems(
                Update,
                (
                    forward_menu_events.in_set(WriteAppCommands),
                    close_with_confirmation,
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

    MenuEvent::set_event_handler(Some(|event: MenuEvent| {
        PENDING_MENU_EVENTS.lock().push(event.id.0.clone());
    }));

    world.insert_non_send_resource(OsMenuResource(menu));
}

fn forward_menu_events(world: &mut World) {
    let drained = {
        let mut events = PENDING_MENU_EVENTS.lock();
        if events.is_empty() {
            return;
        }
        std::mem::take(&mut *events)
    };

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

/// Replacement for bevy's `close_when_requested` that shows a confirmation
/// dialog when terminals are still running. Defers the dialog to the
/// exclusive `show_pending_close_dialogs` system to avoid deadlocks.
fn close_with_confirmation(
    mut closed: MessageReader<WindowCloseRequested>,
    mut windows: Query<&mut Window>,
    settings: Res<AppSettings>,
    live_terminals: Query<(), (With<Terminal>, Without<PtyExited>)>,
    mut pending: ResMut<PendingWindowClose>,
) {
    for event in closed.read() {
        let should_confirm = terminal::should_confirm_close(&settings);
        if should_confirm && live_terminals.iter().count() > 0 {
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
}
