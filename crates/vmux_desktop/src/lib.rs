mod browser;
mod command;
pub(crate) mod keybinding;
mod layout;
mod os_menu;
mod scene;
mod settings;
mod unit;

use bevy::asset::io::web::WebAssetPlugin;
use bevy::prelude::*;
use bevy::window::{CompositeAlphaMode, Window as NativeWindow, WindowPlugin};

use {
    browser::BrowserPlugin, command::CommandPlugin, keybinding::KeyBindingPlugin,
    layout::LayoutPlugin, os_menu::OsMenuPlugin, scene::ScenePlugin, settings::SettingsPlugin,
    vmux_header::HeaderPlugin, vmux_side_sheet::SideSheetWebviewPlugin,
    vmux_webview_app::WebviewAppRegistryPlugin,
};

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
            CommandPlugin,
            KeyBindingPlugin,
            ScenePlugin,
            OsMenuPlugin,
            WebviewAppRegistryPlugin,
            HeaderPlugin,
            SideSheetWebviewPlugin,
            BrowserPlugin,
            LayoutPlugin,
        ));
    }
}
