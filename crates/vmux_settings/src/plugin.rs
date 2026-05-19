pub mod runtime;
pub mod view;

use bevy::prelude::*;
use bevy_cef::prelude::BinJsEmitEventPlugin;
use vmux_command::{ReadAppCommands, WriteAppCommands};
use vmux_webview_app::WebviewAppRegistry;

use crate::event::SettingsCommandEvent;
use runtime::{
    LastSelfWriteHash, SettingsLoadSet, SettingsWriteRequest, load_settings,
    persist_settings_to_disk, reload_settings_on_change, update_effective_startup_url,
};
use view::{
    broadcast_schema_to_views, broadcast_settings_to_views, handle_open_settings_command,
    on_settings_command, register_settings_webview_app, reset_sent_markers_on_ui_ready,
};

pub struct SettingsPlugin;

impl Plugin for SettingsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LastSelfWriteHash>()
            .add_message::<SettingsWriteRequest>()
            .configure_sets(
                Startup,
                SettingsLoadSet.before(vmux_layout::LayoutStartupSet::Window),
            )
            .init_resource::<vmux_layout::settings::EffectiveStartupUrl>()
            .add_systems(Startup, load_settings.in_set(SettingsLoadSet))
            .add_systems(
                Startup,
                update_effective_startup_url
                    .after(SettingsLoadSet)
                    .before(vmux_layout::LayoutStartupSet::Post),
            )
            .add_systems(
                Update,
                (persist_settings_to_disk, reload_settings_on_change).chain(),
            )
            .add_systems(Update, update_effective_startup_url);

        register_settings_webview_app(
            app.world_mut()
                .resource_mut::<WebviewAppRegistry>()
                .as_mut(),
        );
        app.add_plugins(BinJsEmitEventPlugin::<SettingsCommandEvent>::default())
            .add_observer(on_settings_command)
            .add_observer(reset_sent_markers_on_ui_ready)
            .add_systems(
                Update,
                (broadcast_schema_to_views, broadcast_settings_to_views),
            )
            .add_systems(
                Update,
                handle_open_settings_command
                    .in_set(ReadAppCommands)
                    .after(WriteAppCommands),
            );
    }
}
