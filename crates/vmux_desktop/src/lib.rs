// Bevy systems inherently use many parameters and complex query types.
#![allow(
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::new_ret_no_self
)]

mod background_lifecycle;
mod browser;
mod os_menu;
mod persistence;

pub(crate) mod shortcut;
mod tray;
pub mod updater;
use bevy::asset::io::web::WebAssetPlugin;
use bevy::prelude::*;
use bevy::window::{CompositeAlphaMode, ExitCondition, Window as NativeWindow, WindowPlugin};
use bevy::winit::WinitSettings;
use std::time::Duration;

use {
    browser::BrowserPlugin, os_menu::OsMenuPlugin, persistence::PersistencePlugin,
    shortcut::ShortcutPlugin, vmux_command::CommandPlugin, vmux_layout::LayoutPlugin,
    vmux_layout::cef::LayoutCefPlugin, vmux_layout::command_bar::plugin::CommandBarPagePlugin,
    vmux_server::ServerPlugin, vmux_service::plugin::ServicePlugin, vmux_setting::SettingsPlugin,
    vmux_space::SpacePlugin, vmux_terminal::TerminalPlugin,
};

use vmux_agent::AgentPlugin;

pub struct VmuxPlugin;

fn primary_window_config(title: String) -> NativeWindow {
    NativeWindow {
        title,
        transparent: true,
        composite_alpha_mode: CompositeAlphaMode::PostMultiplied,
        decorations: true,
        titlebar_shown: true,
        titlebar_transparent: true,
        titlebar_show_title: false,
        titlebar_show_buttons: false,
        movable_by_window_background: false,
        fullsize_content_view: true,
        ime_enabled: true,
        ..default()
    }
}

impl Plugin for VmuxPlugin {
    fn build(&self, app: &mut App) {
        let title = match env!("VMUX_BUILD_PROFILE") {
            "release" => "Vmux".to_string(),
            "local" => format!("Vmux ({})", env!("VMUX_GIT_HASH")),
            "dev" => format!("Vmux Dev ({})", env!("VMUX_GIT_HASH")),
            other => format!("Vmux ({})", other),
        };

        let primary_window = primary_window_config(title);
        let window_plugin = WindowPlugin {
            primary_window: Some(primary_window),
            close_when_requested: false,
            exit_condition: ExitCondition::DontExit,
            ..default()
        };

        // Continuous while focused: drives the bevy_cef external BeginFrame system
        // every Bevy update so CEF paints align with host display refresh.
        app.insert_resource(WinitSettings {
            focused_mode: bevy::winit::UpdateMode::Continuous,
            unfocused_mode: bevy::winit::UpdateMode::reactive_low_power(Duration::from_secs(1)),
        })
        .add_plugins((
            vmux_core::CorePlugin,
            DefaultPlugins
                .set(WebAssetPlugin {
                    silence_startup_warning: true,
                })
                .set(window_plugin)
                .set(bevy::log::LogPlugin {
                    filter: "bevy_camera_controller=warn".into(),
                    ..default()
                }),
            ServerPlugin,
            SettingsPlugin,
            CommandPlugin,
            ShortcutPlugin,
            OsMenuPlugin,
            TerminalPlugin,
            ServicePlugin,
            SpacePlugin,
            LayoutCefPlugin,
            CommandBarPagePlugin,
            BrowserPlugin,
        ))
        .add_plugins((
            AgentPlugin,
            vmux_agent::PageAgentPlugin,
            PersistencePlugin,
            LayoutPlugin,
            updater::VmuxUpdater::builder().build().plugin(),
            background_lifecycle::BackgroundLifecyclePlugin,
            tray::TrayPlugin,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primary_window_enables_ime_input() {
        let window = primary_window_config("Vmux".to_string());

        assert!(window.ime_enabled);
    }

    #[test]
    fn window_plugin_keeps_app_alive_after_last_window_closes() {
        let source = include_str!("lib.rs");
        assert!(
            source.contains("ExitCondition::DontExit"),
            "WindowPlugin must opt out of automatic exit so Vmux.app survives last-window-close"
        );
    }

    #[test]
    fn desktop_uses_single_layout_crate_for_chrome_and_layout() {
        let source = include_str!("lib.rs");

        assert!(source.contains("vmux_layout::"));
        assert!(!source.contains(&["vmux_layout", "::footer"].concat()));
        assert!(!source.contains(&["vmux_", "header::HeaderPlugin"].concat()));
        assert!(!source.contains(&["vmux_", "side_sheet::SideSheetPlugin"].concat()));
    }
}
