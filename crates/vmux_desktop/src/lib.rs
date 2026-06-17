// Bevy systems inherently use many parameters and complex query types.
#![allow(
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::new_ret_no_self
)]

mod background_lifecycle;
mod boot_status;
mod display;
#[cfg(target_os = "macos")]
mod event_tap;
#[cfg(target_os = "macos")]
mod focus_native;
#[cfg(target_os = "macos")]
mod glass;
mod log_forward;
#[cfg(target_os = "macos")]
mod native_keyboard;
mod os_menu;
pub mod panic_hook;
mod persistence;

#[cfg(target_os = "macos")]
mod splash;

pub(crate) mod shortcut;
mod tray;
pub mod updater;
use bevy::asset::io::web::WebAssetPlugin;
use bevy::prelude::*;
use bevy::window::{CompositeAlphaMode, ExitCondition, Window as NativeWindow, WindowPlugin};

use {
    os_menu::OsMenuPlugin, persistence::PersistencePlugin, shortcut::ShortcutPlugin,
    vmux_browser::BrowserPlugin, vmux_command::CommandPlugin, vmux_core::page::ServerPlugin,
    vmux_layout::LayoutPlugin, vmux_layout::cef::LayoutCefPlugin,
    vmux_service::plugin::ServicePlugin, vmux_setting::SettingsPlugin, vmux_space::SpacePlugin,
    vmux_terminal::TerminalPlugin,
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
        visible: !cfg!(target_os = "macos"),
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

        app.insert_resource(background_lifecycle::foreground_winit_settings(false))
            .add_plugins((
                vmux_core::CorePlugin,
                DefaultPlugins
                    .set(WebAssetPlugin {
                        silence_startup_warning: true,
                    })
                    .set(window_plugin)
                    .set(bevy::log::LogPlugin {
                        filter: "bevy_camera_controller=warn".into(),
                        custom_layer: crate::log_forward::file_log_layer,
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
                vmux_history::HistoryPlugin,
                vmux_vibe_setup::VibeSetupPlugin,
                LayoutCefPlugin,
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
                display::DisplayPlugin,
            ));

        app.init_resource::<boot_status::SplashStatus>()
            .init_resource::<boot_status::RestoreComplete>()
            .add_systems(
                Update,
                boot_status::compute_boot_status.after(vmux_layout::stack::ComputeFocusSet),
            );

        #[cfg(target_os = "macos")]
        app.add_plugins((glass::GlassPlugin, splash::SplashPlugin))
            .add_systems(Last, focus_native::apply_winit_host_focus);
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
    fn primary_window_starts_hidden_on_macos_until_backdrop_is_ready() {
        let window = primary_window_config("Vmux".to_string());

        assert_eq!(window.visible, !cfg!(target_os = "macos"));
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
    fn desktop_uses_single_layout_crate_for_cef_and_layout() {
        let source = include_str!("lib.rs");

        assert!(source.contains("vmux_layout::"));
        assert!(!source.contains(&["vmux_layout", "::footer"].concat()));
        assert!(!source.contains(&["vmux_", "header::HeaderPlugin"].concat()));
        assert!(!source.contains(&["vmux_", "side_sheet::SideSheetPlugin"].concat()));
    }

    #[test]
    fn dev_build_has_no_tick_logger() {
        let source = include_str!("lib.rs");

        assert!(!source.contains(&["app", ".update", "():"].concat()));
    }
}
