use crate::{
    os_menu::OsMenuPlugin, permission::PermissionsPlugin, runtime::RuntimePlugin,
    shortcut::ShortcutPlugin,
};
use bevy::app::PluginGroupBuilder;
use bevy::prelude::*;

pub struct DesktopPluginGroup;

impl PluginGroup for DesktopPluginGroup {
    fn build(self) -> PluginGroupBuilder {
        #[allow(unused_mut)]
        let mut builder = PluginGroupBuilder::start::<Self>()
            .add(crate::window::WindowPlugin)
            .add(crate::boot_status::BootStatusPlugin)
            .add(RuntimePlugin)
            .add(PermissionsPlugin)
            .add(OsMenuPlugin)
            .add(ShortcutPlugin)
            .add(crate::relaunch::RelaunchPlugin);

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

        #[cfg(feature = "screenshots")]
        {
            builder = builder.add(crate::screenshot::ScreenshotPlugin);
        }

        #[cfg(not(feature = "screenshots"))]
        {
            builder = builder.add(crate::disabled_features::ScreenshotsDisabledPlugin);
        }

        #[cfg(feature = "recording")]
        {
            builder = builder.add(crate::recording::RecordingPlugin);
        }

        #[cfg(not(feature = "recording"))]
        {
            builder = builder.add(crate::disabled_features::RecordingDisabledPlugin);
        }

        #[cfg(feature = "updater")]
        {
            builder = builder.add(crate::updater::UpdatePlugin::default());
        }

        #[cfg(not(feature = "updater"))]
        {
            builder = builder.add(crate::disabled_features::UpdaterDisabledPlugin);
        }

        #[cfg(feature = "tray")]
        {
            builder = builder.add(crate::tray::TrayPlugin);
        }

        builder
    }
}
