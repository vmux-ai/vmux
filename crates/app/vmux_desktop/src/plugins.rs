use crate::{
    display::DisplayPlugin, os_menu::OsMenuPlugin, permission::PermissionsPlugin,
    remote::RemotePlugin, runtime::RuntimePlugin, shortcut::ShortcutPlugin,
    window_state::WindowStatePlugin,
};
use bevy::app::PluginGroupBuilder;
use bevy::prelude::*;

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
            crate::window_manager::WindowManagerPlugin,
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
