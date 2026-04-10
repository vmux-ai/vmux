use crate::command::{AppCommand, WriteAppCommands};
use bevy::ecs::message::Messages;
use bevy::prelude::*;
use muda::{Menu, MenuEvent};
use parking_lot::Mutex;
use std::sync::LazyLock;

static PENDING_MENU_EVENTS: LazyLock<Mutex<Vec<String>>> = LazyLock::new(|| Mutex::new(Vec::new()));

#[allow(dead_code)]
struct NativeMenuResource(Menu);

pub struct NativeMenuPlugin;

impl Plugin for NativeMenuPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup).add_systems(
            Update,
            forward_menu_events.in_set(WriteAppCommands),
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

    world.insert_non_send_resource(NativeMenuResource(menu));
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
        if let Some(cmd) = AppCommand::from_menu_id(event_id.as_str()) {
            world.resource_mut::<Messages<AppCommand>>().write(cmd);
        } else {
            warn!(len = event_id.len(), "unknown native menu item");
        }
    }
}
