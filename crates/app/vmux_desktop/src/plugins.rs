use crate::{
    display::DisplayPlugin, os_menu::OsMenuPlugin, permission::PermissionsPlugin,
    persistence::PersistencePlugin, remote::RemotePlugin, runtime::RuntimePlugin,
    shortcut::ShortcutPlugin, tools::ToolsPlugin, window_state::WindowStatePlugin,
};
use bevy::app::PluginGroupBuilder;
use bevy::prelude::*;

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
            .add(vmux_start::StartPlugin)
            .add(ToolsPlugin)
    }
}
