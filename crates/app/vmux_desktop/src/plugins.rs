//! The plugin groups that [`crate::VmuxPlugin`] composes.
//!
//! Ordering constraint: `BrowserPlugin` snapshots every spawned `PageManifest` into
//! `CefEmbeddedHosts` while it builds, so every group that registers a page — [`FeaturePlugins`]
//! and `LayoutPlugin` — must be added before it.

use crate::{
    display::DisplayPlugin, os_menu::OsMenuPlugin, permission::PermissionsPlugin,
    persistence::PersistencePlugin, remote::RemotePlugin, runtime::RuntimePlugin,
    shortcut::ShortcutPlugin, tools::ToolsPlugin, window_state::WindowStatePlugin,
};
use bevy::app::PluginGroupBuilder;
use bevy::prelude::*;

/// Foundation plugins: shared type registration, the page server, command routing, settings,
/// and session persistence. Everything else assumes these are present.
pub struct VmuxCorePlugins;

impl PluginGroup for VmuxCorePlugins {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add(vmux_flex::FlexPlugin)
            .add(vmux_core::CorePlugin)
            .add(vmux_core::page::PagePlugin)
            .add(vmux_command::CommandPlugin)
            .add(vmux_setting::SettingsPlugin)
            .add(PersistencePlugin)
    }
}

/// The OS-facing layer of the desktop app: window and display management, wake and render
/// pacing, the native menu bar and tray, global shortcuts, permissions, phone pairing, and
/// updates.
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
            .add(MediaPlugin)
            .add(RemotePlugin)
            .add(UpdaterPlugin);

        #[cfg(feature = "native-notifications")]
        {
            builder = builder.add(crate::notify::NotificationPlugin);
        }

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
        app.add_plugins((
            WindowStatePlugin,
            DisplayPlugin,
            crate::appearance::DesktopAppearancePlugin,
            crate::boot_status::BootStatusPlugin,
        ));

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
        app.add_plugins(crate::screenshot::ScreenshotPlugin);

        #[cfg(not(feature = "screenshots"))]
        app.add_plugins(crate::disabled_features::ScreenshotsDisabledPlugin);

        #[cfg(feature = "recording")]
        app.add_plugins(crate::recording::RecordingPlugin);

        #[cfg(not(feature = "recording"))]
        app.add_plugins(crate::disabled_features::RecordingDisabledPlugin);
    }
}

/// Checks for and installs releases, and restarts the app to apply them. Restart is also
/// reachable on its own from the debug, extensions, and layout pages.
struct UpdaterPlugin;

impl Plugin for UpdaterPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(crate::relaunch::RelaunchPlugin);

        #[cfg(feature = "updater")]
        app.add_plugins(crate::updater::VmuxUpdater::builder().build().plugin());

        #[cfg(not(feature = "updater"))]
        app.add_plugins(crate::disabled_features::UpdaterDisabledPlugin);
    }
}

/// The user-facing feature domains, each owned by its crate. Every plugin here may register
/// pages, so the group must be added before `BrowserPlugin`.
///
/// `KeyStrokePlugin` comes first and is owned by no domain: keystrokes reach several of these
/// crates, and registering the shared type once here is what stops two of them registering it and
/// delivering every key twice.
pub struct FeaturePlugins;

impl PluginGroup for FeaturePlugins {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add(vmux_core::input::KeyStrokePlugin)
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
