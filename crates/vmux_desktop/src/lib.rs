mod command;
mod layout3;
mod menu;
mod rounded;
mod scene;
mod settings;
mod unit;

use bevy::asset::io::web::WebAssetPlugin;
use bevy::prelude::*;
use bevy::window::{CompositeAlphaMode, Window as NativeWindow, WindowPlugin};
#[cfg(target_os = "macos")]
use bevy::winit::WinitSettings;

use {
    // browser::BrowserPlugin,
    command::CommandPlugin,
    layout3::Layout3Plugin,
    menu::NativeMenuPlugin,
    rounded::RoundedMaterialPlugin,
    scene::ScenePlugin,
    settings::SettingsPlugin,
};
// use vmux_history::HistoryPlugin;

pub struct VmuxPlugin;

impl Plugin for VmuxPlugin {
    fn build(&self, app: &mut App) {
        let primary_window = NativeWindow {
            transparent: true,
            composite_alpha_mode: CompositeAlphaMode::PostMultiplied,
            decorations: true,
            titlebar_shown: false,
            movable_by_window_background: true,
            fullsize_content_view: true,
            ..default()
        };
        let window_plugin = WindowPlugin {
            primary_window: Some(primary_window),
            ..default()
        };

        app.add_plugins((
            DefaultPlugins
                .set(WebAssetPlugin {
                    silence_startup_warning: true,
                })
                .set(window_plugin),
            RoundedMaterialPlugin,
            SettingsPlugin,
            CommandPlugin,
            ScenePlugin,
            NativeMenuPlugin,
            Layout3Plugin,
            // BrowserPlugin,
            // HistoryPlugin,
        ));

        #[cfg(target_os = "macos")]
        app.insert_resource(WinitSettings::desktop_app());
    }
}
