use crate::{
    os_menu::OsMenuPlugin, permission::PermissionsPlugin, remote::RemotePlugin,
    runtime::RuntimePlugin, shortcut::ShortcutPlugin,
};
use bevy::app::PluginGroupBuilder;
use bevy::prelude::*;

pub struct DesktopPluginGroup;

impl PluginGroup for DesktopPluginGroup {
    fn build(self) -> PluginGroupBuilder {
        #[allow(unused_mut)]
        let mut builder = PluginGroupBuilder::start::<Self>()
            .add(crate::window::WindowPlugin)
            .add(crate::appearance::DesktopAppearancePlugin)
            .add(crate::boot_status::BootStatusPlugin)
            .add(RuntimePlugin)
            .add(PermissionsPlugin)
            .add(OsMenuPlugin)
            .add(ShortcutPlugin)
            .add(MediaPlugin)
            .add(RemotePlugin)
            .add(UpdaterPlugin);

        #[cfg(target_os = "macos")]
        {
            builder = builder.add(vmux_input::KeyboardPlugin);
        }

        #[cfg(all(target_os = "macos", feature = "native-glass"))]
        {
            builder = builder
                .add(crate::glass::GlassPlugin)
                .add(crate::splash::SplashPlugin);
        }

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
        app.add_plugins(crate::updater::UpdatePlugin::default());

        #[cfg(not(feature = "updater"))]
        app.add_plugins(crate::disabled_features::UpdaterDisabledPlugin);
    }
}
