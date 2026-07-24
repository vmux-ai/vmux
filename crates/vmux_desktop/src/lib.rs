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
mod background_lifecycle;
mod bookmark_menu;
mod bookmark_persistence;
mod boot_status;
mod browser_scroll;
mod browser_snapshot;
#[cfg(any(feature = "recording", feature = "screenshots"))]
mod capture_output;
#[cfg(any(
    not(feature = "recording"),
    not(feature = "screenshots"),
    not(feature = "updater")
))]
mod disabled_features;
mod display;
#[cfg(target_os = "macos")]
mod event_tap;
#[cfg(target_os = "macos")]
mod focus_native;
#[cfg(all(target_os = "macos", feature = "native-glass"))]
mod glass;
mod lechat_bridge;
mod log_forward;
mod media_permission;
#[cfg(target_os = "macos")]
mod native_keyboard;
#[cfg(feature = "native-notifications")]
mod notify;
mod os_menu;
pub mod panic_hook;
mod persistence;
#[cfg(feature = "recording")]
mod recording;
mod relaunch;
#[cfg(feature = "screenshots")]
mod screenshot;
mod tool_registry;

#[cfg(all(target_os = "macos", feature = "native-glass"))]
mod splash;

pub(crate) mod shortcut;
#[cfg(feature = "tray")]
mod tray;
#[cfg(feature = "updater")]
pub mod updater;
mod window_state;
use bevy::asset::io::web::WebAssetPlugin;
use bevy::prelude::*;
use bevy::window::{
    CompositeAlphaMode, ExitCondition, MonitorSelection, Window as NativeWindow, WindowPlugin,
    WindowPosition, WindowResolution,
};

use {
    bookmark_menu::BookmarkMenuPlugin, bookmark_persistence::BookmarkPersistencePlugin,
    os_menu::OsMenuPlugin, persistence::PersistencePlugin, shortcut::ShortcutPlugin,
    vmux_browser::BrowserPlugin, vmux_command::CommandPlugin, vmux_command::WriteAppCommands,
    vmux_core::page::ServerPlugin, vmux_editor::EditorPlugin, vmux_git::GitPlugin,
    vmux_layout::LayoutPlugin, vmux_layout::cef::LayoutCefPlugin,
    vmux_service::plugin::ServicePlugin, vmux_setting::SettingsPlugin, vmux_space::SpacePlugin,
    vmux_terminal::TerminalPlugin,
};

use vmux_agent::AgentPlugin;

/// The top-level aggregator: adds `DefaultPlugins`, every feature plugin, and the
/// macOS-native integrations that make up the desktop app.
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
        visible: !cfg!(all(target_os = "macos", feature = "native-glass")),
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

        let winit_settings = background_lifecycle::foreground_winit_settings(false, false, false);
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
                vmux_knowledge::KnowledgePlugin,
                tool_registry::ToolRegistryPlugin,
                vmux_agent::AgentChatPagePlugin,
                vmux_agent::AgentsManagerPlugin,
                vmux_agent::AgentSetupPlugin,
                vmux_layout::start::StartPlugin,
                LayoutCefPlugin,
                vmux_browser::ExtensionsPlugin,
                BrowserPlugin,
                media_permission::MediaPermissionPlugin,
                lechat_bridge::LeChatBridgePlugin,
            ))
            .add_plugins((
                AgentPlugin,
                vmux_agent::PageAgentPlugin,
                vmux_agent::AcpAgentPlugin,
                PersistencePlugin,
                BookmarkPersistencePlugin,
                BookmarkMenuPlugin,
                LayoutPlugin,
                background_lifecycle::BackgroundLifecyclePlugin,
                display::DisplayPlugin,
                relaunch::RelaunchPlugin,
                window_state::WindowStatePlugin,
            ));

        #[cfg(feature = "updater")]
        app.add_plugins(updater::VmuxUpdater::builder().build().plugin());

        #[cfg(not(feature = "updater"))]
        app.add_systems(Startup, disabled_features::mark_updater_unavailable)
            .add_systems(Update, disabled_features::reject_update_checks);

        #[cfg(feature = "tray")]
        app.add_plugins(tray::TrayPlugin);

        app.init_resource::<boot_status::SplashStatus>()
            .init_resource::<boot_status::RestoreComplete>()
            .add_systems(
                Update,
                boot_status::compute_boot_status.after(vmux_layout::stack::ComputeFocusSet),
            )
            .add_systems(
                Update,
                (
                    browser_snapshot::drive_pending_nav_snapshots,
                    browser_scroll::run_scrolls,
                    browser_snapshot::start_snapshots,
                    browser_snapshot::shape_snapshot_results,
                )
                    .chain()
                    .after(WriteAppCommands),
            )
            .add_systems(Startup, appearance::seed_system_appearance);

        #[cfg(feature = "screenshots")]
        app.init_resource::<screenshot::ScreenshotBridge>()
            .add_systems(
                Update,
                (screenshot::start_screenshots, screenshot::drain_screenshots)
                    .chain()
                    .after(WriteAppCommands),
            );

        #[cfg(not(feature = "screenshots"))]
        app.add_systems(
            Update,
            disabled_features::reject_screenshots.after(WriteAppCommands),
        );

        #[cfg(feature = "recording")]
        app.init_resource::<recording::RecordingBridge>()
            .init_resource::<recording::RecordingStatus>()
            .add_message::<recording::RecordingControl>()
            .add_systems(
                Update,
                (
                    recording::start_recording,
                    recording::handle_recording_control,
                    recording::auto_stop_recordings,
                    recording::drain_recordings,
                )
                    .chain()
                    .after(WriteAppCommands),
            );

        #[cfg(not(feature = "recording"))]
        app.add_systems(
            Update,
            (
                disabled_features::reject_recording_starts,
                disabled_features::reject_recording_stops,
            )
                .after(WriteAppCommands),
        );

        #[cfg(feature = "native-notifications")]
        app.add_systems(Startup, notify::request_notification_auth)
            .add_systems(Update, notify::post_os_notifications);

        #[cfg(all(target_os = "macos", feature = "native-glass"))]
        app.add_plugins((glass::GlassPlugin, splash::SplashPlugin));

        #[cfg(target_os = "macos")]
        app.add_systems(Last, focus_native::apply_winit_host_focus);
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
