// Bevy systems inherently use many parameters and complex query types.
#![allow(
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::new_ret_no_self
)]

mod background_lifecycle;
mod boot_status;
mod browser_scroll;
mod browser_snapshot;
mod display;
#[cfg(target_os = "macos")]
mod event_tap;
#[cfg(target_os = "macos")]
mod focus_native;
#[cfg(target_os = "macos")]
mod glass;
mod lechat_bridge;
mod log_forward;
mod media_permission;
#[cfg(target_os = "macos")]
mod native_keyboard;
mod notify;
mod os_menu;
pub mod panic_hook;
mod persistence;
mod recording;
mod screenshot;

#[cfg(target_os = "macos")]
mod splash;

pub(crate) mod shortcut;
mod tray;
pub mod updater;
mod window_state;
use bevy::asset::io::web::WebAssetPlugin;
use bevy::prelude::*;
use bevy::window::{
    CompositeAlphaMode, ExitCondition, MonitorSelection, Window as NativeWindow, WindowPlugin,
    WindowPosition, WindowResolution,
};

use {
    os_menu::OsMenuPlugin, persistence::PersistencePlugin, shortcut::ShortcutPlugin,
    vmux_browser::BrowserPlugin, vmux_command::CommandPlugin, vmux_command::WriteAppCommands,
    vmux_core::page::ServerPlugin, vmux_editor::EditorPlugin, vmux_git::GitPlugin,
    vmux_layout::LayoutPlugin, vmux_layout::cef::LayoutCefPlugin,
    vmux_service::plugin::ServicePlugin, vmux_setting::SettingsPlugin, vmux_space::SpacePlugin,
    vmux_terminal::TerminalPlugin,
};

use vmux_agent::AgentPlugin;

pub struct VmuxPlugin;

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
        ime_enabled: true,
        visible: !cfg!(target_os = "macos"),
        position: WindowPosition::Centered(MonitorSelection::Primary),
        resolution: WindowResolution::new(DEFAULT_WINDOW_WIDTH, DEFAULT_WINDOW_HEIGHT),
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

        let winit_settings = background_lifecycle::foreground_winit_settings(false, false);
        app.insert_resource(winit_settings)
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
                EditorPlugin,
                GitPlugin,
                ServicePlugin,
                SpacePlugin,
            ))
            .add_plugins((
                vmux_team::TeamPlugin,
                vmux_history::HistoryPlugin,
                vmux_agent::vibe::setup::AgentSetupPlugin,
                LayoutCefPlugin,
                vmux_browser::ExtensionsPlugin,
                BrowserPlugin,
                media_permission::MediaPermissionPlugin,
                lechat_bridge::LeChatBridgePlugin,
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
                window_state::WindowStatePlugin,
            ));

        app.init_resource::<boot_status::SplashStatus>()
            .init_resource::<boot_status::RestoreComplete>()
            .init_resource::<screenshot::ScreenshotBridge>()
            .init_resource::<recording::RecordingBridge>()
            .init_resource::<recording::RecordingStatus>()
            .add_message::<recording::RecordingControl>()
            .add_systems(
                Update,
                boot_status::compute_boot_status.after(vmux_layout::stack::ComputeFocusSet),
            )
            .add_systems(
                Update,
                (
                    screenshot::start_screenshots,
                    screenshot::drain_screenshots,
                    browser_snapshot::drive_pending_nav_snapshots,
                    browser_scroll::run_scrolls,
                    browser_snapshot::start_snapshots,
                    browser_snapshot::shape_snapshot_results,
                    recording::start_recording,
                    recording::handle_recording_control,
                    recording::auto_stop_recordings,
                    recording::drain_recordings,
                )
                    .chain()
                    .after(WriteAppCommands),
            )
            .add_systems(Startup, notify::request_notification_auth)
            .add_systems(Update, notify::post_os_notifications);

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
