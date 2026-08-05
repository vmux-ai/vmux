//! The plugin groups that [`crate::VmuxPlugin`] composes.
//!
//! Ordering constraint: `BrowserPlugin` snapshots every spawned `PageManifest` into
//! `CefEmbeddedHosts` while it builds, so every group that registers a page — [`FeaturePlugins`]
//! and `LayoutPlugin` — must be added before it.

use bevy::app::{PluginGroup, PluginGroupBuilder};

use crate::{
    background_lifecycle::BackgroundLifecyclePlugin, display::DisplayPlugin,
    lechat_bridge::LeChatBridgePlugin, media_permission::MediaPermissionPlugin,
    os_menu::OsMenuPlugin, persistence::PersistencePlugin, relaunch::RelaunchPlugin,
    shortcut::ShortcutPlugin, tools::ToolsPlugin, window_state::WindowStatePlugin,
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

/// The OS-facing layer of the desktop app: window and display management, background
/// lifecycle, the native menu bar and tray, global shortcuts, permissions, and updates.
pub struct DesktopPlugins;

impl PluginGroup for DesktopPlugins {
    fn build(self) -> PluginGroupBuilder {
        #[allow(unused_mut)]
        let mut builder = PluginGroupBuilder::start::<Self>()
            .add(WindowStatePlugin)
            .add(DisplayPlugin)
            .add(BackgroundLifecyclePlugin)
            .add(RelaunchPlugin)
            .add(MediaPermissionPlugin)
            .add(OsMenuPlugin)
            .add(ShortcutPlugin)
            .add(LeChatBridgePlugin);

        #[cfg(feature = "tray")]
        {
            builder = builder.add(crate::tray::TrayPlugin);
        }

        #[cfg(feature = "updater")]
        {
            builder = builder.add(crate::updater::VmuxUpdater::builder().build().plugin());
        }

        #[cfg(all(target_os = "macos", feature = "native-glass"))]
        {
            builder = builder
                .add(crate::glass::GlassPlugin)
                .add(crate::splash::SplashPlugin);
        }

        builder
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
