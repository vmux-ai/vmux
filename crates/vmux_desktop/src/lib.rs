mod browser;
mod layout;
mod scene;
mod settings;

use bevy::asset::io::web::WebAssetPlugin;
use bevy::prelude::*;
use bevy::window::{CompositeAlphaMode, Window as NativeWindow, WindowPlugin};

use crate::scene::ScenePlugin;
use browser::BrowserPlugin;
use layout::LayoutPlugin;
use settings::SettingsPlugin;
use vmux_history::HistoryPlugin;

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
            SettingsPlugin,
            ScenePlugin,
            LayoutPlugin,
            BrowserPlugin,
            HistoryPlugin,
        ));
    }
}
