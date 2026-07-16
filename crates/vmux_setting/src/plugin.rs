pub mod runtime;
pub mod view;

use bevy::{ecs::message::MessageReader, prelude::*};
use bevy_cef::prelude::{BinEventEmitterPlugin, WebviewExtendStandardMaterial};
use vmux_command::{ReadAppCommands, WriteAppCommands};

use crate::event::{
    CheckForUpdatesEvent, CheckForUpdatesRequest, CurrentUpdateCheckStatus, SettingsCommandEvent,
};
use runtime::{
    LastSelfWriteHash, SettingsLoadSet, SettingsSaveDebounce, SettingsSaveRequest,
    SettingsWriteRequest, flush_settings_save, load_settings, persist_settings_to_disk,
    reload_settings_on_change, request_settings_save,
};
use view::{
    broadcast_schema_to_views, broadcast_settings_to_views, broadcast_update_status_to_views,
    handle_open_settings_command, handle_settings_page_open, on_check_for_updates,
    on_settings_command, reset_sent_markers_on_page_ready,
};

/// Wires settings: RON load/save with debounce, schema and settings broadcasts, and the
/// settings webview.
pub struct SettingsPlugin;

impl Plugin for SettingsPlugin {
    fn build(&self, app: &mut App) {
        app.world_mut().spawn(crate::PAGE_MANIFEST);
        vmux_core::register_host_spawn(app, "settings");
        app.init_resource::<LastSelfWriteHash>()
            .init_resource::<SettingsSaveDebounce>()
            .init_resource::<CurrentUpdateCheckStatus>()
            .add_message::<SettingsWriteRequest>()
            .add_message::<SettingsSaveRequest>()
            .add_message::<CheckForUpdatesRequest>()
            .configure_sets(
                Startup,
                SettingsLoadSet.before(vmux_layout::LayoutStartupSet::Window),
            )
            .init_resource::<vmux_layout::settings::EffectiveStartupUrl>()
            .init_resource::<crate::appearance::SystemAppearance>()
            .init_resource::<crate::appearance::ResolvedColorScheme>()
            .add_message::<crate::appearance::ColorSchemeChanged>()
            .add_systems(
                Update,
                (
                    crate::appearance::track_window_theme,
                    crate::appearance::update_resolved_color_scheme,
                )
                    .chain(),
            )
            .add_systems(Startup, load_settings.in_set(SettingsLoadSet))
            .add_systems(
                Update,
                (
                    request_settings_save,
                    flush_settings_save,
                    persist_settings_to_disk,
                    reload_settings_on_change,
                )
                    .chain(),
            )
            .add_message::<vmux_core::page::SettingsPageSpawnRequest>()
            .add_systems(Update, respond_settings_spawn.in_set(ReadAppCommands))
            .add_systems(
                Update,
                handle_settings_page_open.in_set(vmux_core::PageOpenSet::HandleKnownPages),
            )
            .add_plugins(BinEventEmitterPlugin::<(
                SettingsCommandEvent,
                CheckForUpdatesEvent,
            )>::for_hosts(&["settings"]))
            .add_observer(on_settings_command)
            .add_observer(on_check_for_updates)
            .add_observer(reset_sent_markers_on_page_ready)
            .add_systems(
                Update,
                (
                    broadcast_schema_to_views,
                    broadcast_settings_to_views,
                    broadcast_update_status_to_views,
                ),
            )
            .add_systems(
                Update,
                handle_open_settings_command
                    .in_set(ReadAppCommands)
                    .after(WriteAppCommands),
            );
    }
}

fn respond_settings_spawn(
    mut reader: MessageReader<vmux_core::page::SettingsPageSpawnRequest>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    for req in reader.read() {
        let entity = commands
            .spawn(view::Settings::new(&mut meshes, &mut webview_mt))
            .id();
        commands.entity(entity).insert(ChildOf(req.target_stack));
    }
}
