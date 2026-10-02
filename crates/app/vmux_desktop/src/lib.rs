#![allow(
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::new_ret_no_self
)]

use bevy::prelude::*;
use bevy::window::{
    CompositeAlphaMode, ExitCondition, MonitorSelection, Window as NativeWindow, WindowPlugin,
    WindowPosition, WindowResolution,
};

use crate::plugin::DesktopPluginGroup;

mod boot_status;
#[cfg(any(feature = "recording", feature = "screenshots"))]
mod capture_output;
#[cfg(any(
    not(feature = "recording"),
    not(feature = "screenshots"),
    not(feature = "updater")
))]
mod disabled_features;
#[cfg(all(target_os = "macos", feature = "native-glass"))]
mod glass;
mod log_forward;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(feature = "native-notifications")]
mod notify;
mod os_menu;
pub mod panic_hook;
mod permission;
mod plugin;
#[cfg(feature = "recording")]
mod recording;
mod relaunch;
mod runtime;
#[cfg(feature = "screenshots")]
mod screenshot;
#[cfg(all(target_os = "macos", feature = "native-glass"))]
mod splash;

#[cfg(feature = "tray")]
mod tray;
#[cfg(feature = "updater")]
pub mod updater;
mod window;
#[cfg(any(target_os = "macos", test))]
mod window_interaction;

pub struct VmuxPlugin;

impl Plugin for VmuxPlugin {
    fn build(&self, app: &mut App) {
        let winit_settings = runtime::WakePolicy::foreground(false, false);
        app.insert_resource(winit_settings).add_plugins((
            DefaultPlugins
                .set(Self::window())
                .set(bevy::log::LogPlugin {
                    filter: "bevy_camera_controller=warn".into(),
                    custom_layer: crate::log_forward::file_log_layer,
                    ..default()
                }),
            vmux_app::VmuxPlugin::builder().desktop().build(),
            DesktopPluginGroup,
        ));
    }
}

impl VmuxPlugin {
    fn window() -> WindowPlugin {
        WindowPlugin {
            primary_window: Some(Self::window_config(false)),
            close_when_requested: false,
            exit_condition: ExitCondition::DontExit,
            ..default()
        }
    }

    pub(crate) fn window_config(secondary: bool) -> NativeWindow {
        NativeWindow {
            title: Self::window_title(),
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
            position: if secondary {
                WindowPosition::Automatic
            } else {
                WindowPosition::Centered(MonitorSelection::Primary)
            },
            resolution: WindowResolution::new(DEFAULT_WINDOW_WIDTH, DEFAULT_WINDOW_HEIGHT),
            ..default()
        }
    }

    fn window_title() -> String {
        match env!("VMUX_BUILD_PROFILE") {
            "release" => "Vmux".to_string(),
            "local" => format!("Vmux ({})", env!("VMUX_GIT_HASH")),
            "dev" => format!("Vmux Dev ({})", env!("VMUX_GIT_HASH")),
            other => format!("Vmux ({})", other),
        }
    }
}

const DEFAULT_WINDOW_WIDTH: u32 = 1280;
const DEFAULT_WINDOW_HEIGHT: u32 = 800;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primary_window_enables_ime_input() {
        let window = VmuxPlugin::window_config(false);

        assert!(window.ime_enabled);
    }

    #[test]
    fn primary_window_starts_hidden_when_native_glass_needs_backdrop_setup() {
        let window = VmuxPlugin::window_config(false);

        assert_eq!(
            window.visible,
            !cfg!(all(target_os = "macos", feature = "native-glass"))
        );
    }

    #[test]
    fn primary_window_defaults_to_centered_default_size() {
        let window = VmuxPlugin::window_config(false);

        assert!(matches!(
            window.position,
            WindowPosition::Centered(MonitorSelection::Primary)
        ));
        assert_eq!(window.resolution.physical_width(), DEFAULT_WINDOW_WIDTH);
        assert_eq!(window.resolution.physical_height(), DEFAULT_WINDOW_HEIGHT);
    }

    #[test]
    fn secondary_window_uses_system_positioning() {
        assert_eq!(
            VmuxPlugin::window_config(true).position,
            WindowPosition::Automatic
        );
    }

    #[test]
    fn window_plugin_keeps_app_alive_after_last_window_closes() {
        let plugin = VmuxPlugin::window();

        assert!(matches!(plugin.exit_condition, ExitCondition::DontExit));
        assert!(!plugin.close_when_requested);
    }
}
