// Bevy systems inherently use many parameters and complex query types.
#![allow(
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::new_ret_no_self
)]

mod browser;
mod command;
mod command_bar;
mod layout;
mod os_menu;
mod persistence;
mod profile;
mod scene;
mod settings;
pub(crate) mod shortcut;
mod terminal;
mod themes;
mod unit;
pub mod updater;

use bevy::asset::io::web::WebAssetPlugin;
use bevy::prelude::*;
use bevy::window::{CompositeAlphaMode, Window as NativeWindow, WindowPlugin};
use bevy::winit::WinitSettings;
use std::time::Duration;

use {
    browser::BrowserPlugin, command::CommandPlugin, command_bar::CommandBarInputPlugin,
    layout::LayoutPlugin, os_menu::OsMenuPlugin, persistence::PersistencePlugin,
    profile::ProfilePlugin, scene::ScenePlugin, settings::SettingsPlugin, shortcut::ShortcutPlugin,
    terminal::TerminalInputPlugin, vmux_command_bar::CommandBarPlugin, vmux_header::HeaderPlugin,
    vmux_side_sheet::SideSheetPlugin, vmux_terminal::TerminalPlugin,
    vmux_webview_app::WebviewAppRegistryPlugin,
};

pub struct VmuxPlugin;

impl Plugin for VmuxPlugin {
    fn build(&self, app: &mut App) {
        let title = match env!("VMUX_PROFILE") {
            "release" => "Vmux".to_string(),
            "local" => format!("Vmux ({})", env!("VMUX_GIT_HASH")),
            "dev" => format!("Vmux Dev ({})", env!("VMUX_GIT_HASH")),
            other => format!("Vmux ({})", other),
        };

        let primary_window = NativeWindow {
            title,
            transparent: true,
            composite_alpha_mode: CompositeAlphaMode::PostMultiplied,
            decorations: true,
            titlebar_shown: false,
            movable_by_window_background: false,
            fullsize_content_view: true,
            ..default()
        };
        let window_plugin = WindowPlugin {
            primary_window: Some(primary_window),
            close_when_requested: false,
            ..default()
        };

        // CEF's `on_schedule_message_pump_work` can request delayed work (e.g.
        // 100ms from now).  The wake throttler fires the WakeUp immediately, so
        // by the time Bevy runs the pump the work isn't ready yet.  A short
        // reactive timeout guarantees we re-poll promptly instead of stalling
        // for the default 5-second desktop_app() timeout.
        app.insert_resource(WinitSettings {
            focused_mode: bevy::winit::UpdateMode::reactive(Duration::from_millis(50)),
            unfocused_mode: bevy::winit::UpdateMode::reactive_low_power(Duration::from_secs(1)),
        })
        .add_plugins(vmux_core::CorePlugin)
        .add_plugins((
            DefaultPlugins
                .set(WebAssetPlugin {
                    silence_startup_warning: true,
                })
                .set(window_plugin)
                .set(bevy::log::LogPlugin {
                    filter: "bevy_camera_controller=warn".into(),
                    ..default()
                }),
            SettingsPlugin,
            CommandPlugin,
            ShortcutPlugin,
            ScenePlugin,
            OsMenuPlugin,
            WebviewAppRegistryPlugin,
            HeaderPlugin,
            SideSheetPlugin,
            CommandBarPlugin,
            TerminalPlugin,
            CommandBarInputPlugin,
            BrowserPlugin,
        ))
        .add_plugins((
            TerminalInputPlugin,
            PersistencePlugin,
            ProfilePlugin,
            LayoutPlugin,
            updater::VmuxUpdater::builder().build().plugin(),
        ));
    }
}
