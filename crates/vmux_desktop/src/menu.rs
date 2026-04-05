use crate::command::AppCommand;
use bevy::ecs::system::NonSendMarker;
use bevy::prelude::*;
use muda::{Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};

pub struct NativeMenuPlugin;

impl Plugin for NativeMenuPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup)
            .add_systems(Update, forward_menu_events);
    }
}

fn setup(_main_thread_only: NonSendMarker) {
    let menu = Menu::new();

    let app_menu = Submenu::new("Vmux", true);
    app_menu
        .append_items(&[
            &PredefinedMenuItem::about(None, None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::quit(None),
        ])
        .unwrap();

    let space_menu = Submenu::new("Space", true);
    space_menu
        .append_items(&[&MenuItem::with_id("new_space", "New Space", true, None)])
        .unwrap();

    menu.append_items(&[&app_menu, &space_menu]).unwrap();

    #[cfg(target_os = "macos")]
    menu.init_for_nsapp();
}

fn forward_menu_events(mut writer: MessageWriter<AppCommand>) {
    while let Ok(event) = MenuEvent::receiver().try_recv() {
        match event.id.as_ref() {
            "new_space" => {
                writer.write(AppCommand::NewSpace);
            }
            _ => {
                bevy::log::warn!("Unknown menu item: {:?}", event.id);
            }
        }
    }
}
