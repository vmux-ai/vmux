use crate::command::{AppCommand, WriteAppCommands};
use crate::settings::AppSettings;
use crate::terminal::{self, PtyExited, Terminal};
use bevy::app::AppExit;
use bevy::ecs::message::Messages;
use bevy::prelude::*;
use bevy::window::{ClosingWindow, WindowCloseRequested};
use muda::{Menu, MenuEvent};
use parking_lot::Mutex;
use std::sync::LazyLock;

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
    AppCommand::build_native_root_menu(&mut menu).unwrap();

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
            warn!(len = event_id.len(), "unknown native menu item");
        }
    }
}

fn handle_quit_request(world: &mut World) {
    let should_confirm = world
        .get_resource::<AppSettings>()
        .and_then(|s| s.terminal.as_ref())
        .is_none_or(|t| t.confirm_close);

    if should_confirm {
        let mut query = world.query_filtered::<(), (With<Terminal>, Without<PtyExited>)>();
        let count = query.iter(world).count();

        if count > 0 && !terminal::confirm_quit_dialog(count) {
            return;
        }
    }

    world
        .resource_mut::<Messages<AppExit>>()
        .write(AppExit::Success);
}

/// Replacement for bevy's `close_when_requested` that shows a confirmation
/// dialog when terminals are still running. Defers the dialog to the
/// exclusive `show_pending_close_dialogs` system to avoid deadlocks.
fn close_with_confirmation(
    mut commands: Commands,
    mut closed: MessageReader<WindowCloseRequested>,
    closing: Query<Entity, With<ClosingWindow>>,
    settings: Res<AppSettings>,
    live_terminals: Query<(), (With<Terminal>, Without<PtyExited>)>,
    mut pending: ResMut<PendingWindowClose>,
) {
    // Despawn windows that were marked as closing on the previous frame.
    for window in closing.iter() {
        commands.entity(window).despawn();
    }
    // Process new close requests.
    for event in closed.read() {
        let should_confirm = terminal::should_confirm_close(&settings);
        if should_confirm && live_terminals.iter().count() > 0 {
            // Defer dialog to exclusive system
            pending.window = Some(event.window);
        } else {
            commands.entity(event.window).try_insert(ClosingWindow);
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
    {
        entity_mut.insert(ClosingWindow);
    }
}
