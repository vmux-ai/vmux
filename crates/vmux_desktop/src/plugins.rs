//! The plugin groups that [`crate::VmuxPlugin`] composes.
//!
//! Ordering constraint: `BrowserPlugin` snapshots every spawned `PageManifest` into
//! `CefEmbeddedHosts` while it builds, so every group that registers a page — [`FeaturePlugins`]
//! and `LayoutPlugin` — must be added before it.

use bevy::app::PluginGroupBuilder;
use bevy::prelude::*;
use bevy_cef::prelude::{BinEventEmitterPlugin, JsEmitEventPlugin};
use vmux_command::WriteAppCommands;
use vmux_layout::event::RestartRequestEvent;

use crate::{
    display::DisplayPlugin, os_menu::OsMenuPlugin, permission::PermissionsPlugin,
    persistence::PersistencePlugin, runtime::RuntimePlugin, shortcut::ShortcutPlugin,
    tools::ToolsPlugin, window_state::WindowStatePlugin,
};

/// Foundation plugins: shared type registration, the page server, command routing, settings,
/// and session persistence. Everything else assumes these are present.
pub struct VmuxCorePlugins;

impl PluginGroup for VmuxCorePlugins {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add(vmux_core::CorePlugin)
            .add(vmux_core::page::ServerPlugin)
            .add(vmux_command::CommandPlugin)
            .add(vmux_setting::SettingsPlugin)
            .add(PersistencePlugin)
    }
}

/// The OS-facing layer of the desktop app: window and display management, wake and render
/// pacing, the native menu bar and tray, global shortcuts, permissions, and updates.
pub struct DesktopPlugins;

impl PluginGroup for DesktopPlugins {
    fn build(self) -> PluginGroupBuilder {
        #[allow(unused_mut)]
        let mut builder = PluginGroupBuilder::start::<Self>()
            .add(NativeWindowPlugin)
            .add(RuntimePlugin)
            .add(PermissionsPlugin)
            .add(OsMenuPlugin)
            .add(ShortcutPlugin)
            .add(NotificationPlugin)
            .add(MediaPlugin)
            .add(UpdaterPlugin);

        #[cfg(feature = "tray")]
        {
            builder = builder.add(crate::tray::TrayPlugin);
        }

        builder
    }
}

/// The app's native window from launch onward: the light/dark seed and launch splash, boot
/// phase tracking, the glass backdrop, geometry restore and capture, and relocating the
/// window when its monitor disappears.
///
/// These are one unit rather than several because they already interlock — the splash and the
/// glass reveal both read `SplashStatus`, the glass reveal is what marks the window geometry
/// restored, and `WindowStatePlugin` takes over fullscreen when the glass path is compiled out.
pub(crate) struct NativeWindowPlugin;

impl Plugin for NativeWindowPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((WindowStatePlugin, DisplayPlugin))
            .init_resource::<crate::boot_status::SplashStatus>()
            .init_resource::<crate::boot_status::RestoreComplete>()
            .add_systems(Startup, crate::appearance::seed_system_appearance)
            .add_systems(
                Update,
                crate::boot_status::compute_boot_status.after(vmux_layout::stack::ComputeFocusSet),
            );

        #[cfg(all(target_os = "macos", feature = "native-glass"))]
        app.add_plugins((crate::glass::GlassPlugin, crate::splash::SplashPlugin));
    }
}

/// Captures the app for the agent: still screenshots and screen recordings. Either half can
/// be compiled out, in which case its requests are rejected rather than silently dropped.
struct MediaPlugin;

impl Plugin for MediaPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(feature = "screenshots")]
        app.init_resource::<crate::screenshot::ScreenshotBridge>()
            .add_systems(
                Update,
                (
                    crate::screenshot::start_screenshots,
                    crate::screenshot::drain_screenshots,
                )
                    .chain()
                    .after(WriteAppCommands),
            );

        #[cfg(not(feature = "screenshots"))]
        app.add_systems(
            Update,
            crate::disabled_features::reject_screenshots.after(WriteAppCommands),
        );

        #[cfg(feature = "recording")]
        app.init_resource::<crate::recording::RecordingBridge>()
            .init_resource::<crate::recording::RecordingStatus>()
            .add_message::<crate::recording::RecordingControl>()
            .add_systems(
                Update,
                (
                    crate::recording::start_recording,
                    crate::recording::handle_recording_control,
                    crate::recording::auto_stop_recordings,
                    crate::recording::drain_recordings,
                )
                    .chain()
                    .after(WriteAppCommands),
            );

        #[cfg(not(feature = "recording"))]
        app.add_systems(
            Update,
            (
                crate::disabled_features::reject_recording_starts,
                crate::disabled_features::reject_recording_stops,
            )
                .after(WriteAppCommands),
        );
    }
}

/// Checks for and installs releases, and restarts the app to apply them. Restart is also
/// reachable on its own from the debug, extensions, and layout pages.
struct UpdaterPlugin;

impl Plugin for UpdaterPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(BinEventEmitterPlugin::<(RestartRequestEvent,)>::for_hosts(
            &["debug", "extensions", "layout"],
        ))
        .add_plugins(JsEmitEventPlugin::<crate::relaunch::PageRelaunchRequest>::default())
        .add_observer(crate::relaunch::on_restart_request)
        .add_observer(crate::relaunch::on_page_relaunch);

        #[cfg(feature = "updater")]
        app.add_plugins(crate::updater::VmuxUpdater::builder().build().plugin());

        #[cfg(not(feature = "updater"))]
        app.add_systems(Startup, crate::disabled_features::mark_updater_unavailable)
            .add_systems(Update, crate::disabled_features::reject_update_checks);
    }
}

/// Posts OS notifications when the platform supports them. Authorization is requested by
/// [`PermissionsPlugin`].
struct NotificationPlugin;

impl Plugin for NotificationPlugin {
    fn build(&self, _app: &mut App) {
        #[cfg(feature = "native-notifications")]
        _app.add_systems(Update, crate::notify::post_os_notifications);
    }
}

/// The user-facing feature domains, each owned by its crate. Every plugin here may register
/// pages, so the group must be added before `BrowserPlugin`.
pub struct FeaturePlugins;

impl PluginGroup for FeaturePlugins {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add(vmux_terminal::TerminalPlugin)
            .add(vmux_editor::EditorPlugin)
            .add(vmux_git::GitPlugin)
            .add(vmux_agent::AgentPlugin)
            .add(vmux_knowledge::KnowledgePlugin)
            .add(vmux_history::HistoryPlugin)
            .add(vmux_team::TeamPlugin)
            .add(vmux_space::SpacePlugin)
            .add(vmux_service::plugin::ServicePlugin)
            .add(vmux_layout::start::StartPlugin)
            .add(ToolsPlugin)
    }
}
