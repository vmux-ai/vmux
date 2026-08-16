//! The desktop application: the top-level binary and [`VmuxPlugin`] that wire every crate
//! together, plus macOS-native integrations (glass/blur, event tap, native focus, tray,
//! menu, recording, persistence).

// Bevy systems inherently use many parameters and complex query types.
#![allow(
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::new_ret_no_self
)]

mod appearance;
mod bookmark_menu;
mod bookmark_persistence;
mod boot_status;
#[cfg(any(feature = "recording", feature = "screenshots"))]
mod capture_output;
#[cfg(any(
    not(feature = "recording"),
    not(feature = "screenshots"),
    not(feature = "updater")
))]
mod disabled_features;
mod display;
#[cfg(all(target_os = "macos", feature = "native-glass"))]
mod glass;
mod key_claim;
mod log_forward;
#[cfg(target_os = "macos")]
mod native_keyboard;
#[cfg(feature = "native-notifications")]
mod notify;
mod os_menu;
pub mod panic_hook;
mod permission;
mod persistence;
pub mod plugins;
#[cfg(feature = "recording")]
mod recording;
mod relaunch;
mod remote;
mod runtime;
#[cfg(feature = "screenshots")]
mod screenshot;
mod tools;

#[cfg(all(target_os = "macos", feature = "native-glass"))]
mod splash;

pub(crate) mod shortcut;
#[cfg(feature = "tray")]
mod tray;
#[cfg(feature = "updater")]
pub mod updater;
mod window_state;
use bevy::prelude::*;
use bevy::window::{
    CompositeAlphaMode, ExitCondition, MonitorSelection, Window as NativeWindow, WindowPlugin,
    WindowPosition, WindowResolution,
};

use crate::plugins::{DesktopPlugins, FeaturePlugins, VmuxCorePlugins};
use {vmux_browser::BrowserPlugin, vmux_layout::LayoutPlugin};

/// The top-level aggregator: adds `DefaultPlugins` and the four plugin groups — core,
/// layout, features, browser, and desktop — that make up the app.
pub struct VmuxPlugin;

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

        let winit_settings = runtime::foreground_winit_settings(false, false);
        app.insert_resource(winit_settings).add_plugins((
            VmuxCorePlugins,
            DefaultPlugins.set(window_plugin).set(bevy::log::LogPlugin {
                filter: "bevy_camera_controller=warn".into(),
                custom_layer: crate::log_forward::file_log_layer,
                ..default()
            }),
            LayoutPlugin,
            FeaturePlugins,
            BrowserPlugin,
            DesktopPlugins,
        ));
    }
}

/// First-launch window size (logical px) when no geometry is persisted in
/// `store.ron`. Restored geometry overrides this after load.
const DEFAULT_WINDOW_WIDTH: u32 = 1280;
const DEFAULT_WINDOW_HEIGHT: u32 = 800;

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
        resizable: true,
        ime_enabled: true,
        visible: !cfg!(all(target_os = "macos", feature = "native-glass")),
        position: WindowPosition::Centered(MonitorSelection::Primary),
        resolution: WindowResolution::new(DEFAULT_WINDOW_WIDTH, DEFAULT_WINDOW_HEIGHT),
        ..default()
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
    fn primary_window_starts_hidden_when_native_glass_needs_backdrop_setup() {
        let window = primary_window_config("Vmux".to_string());

        assert_eq!(
            window.visible,
            !cfg!(all(target_os = "macos", feature = "native-glass"))
        );
    }

    #[test]
    fn primary_window_defaults_to_centered_default_size() {
        let window = primary_window_config("Vmux".to_string());

        assert!(matches!(
            window.position,
            WindowPosition::Centered(MonitorSelection::Primary)
        ));
        assert_eq!(window.resolution.physical_width(), DEFAULT_WINDOW_WIDTH);
        assert_eq!(window.resolution.physical_height(), DEFAULT_WINDOW_HEIGHT);
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
