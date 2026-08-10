pub mod runtime;
pub mod view;

use bevy::{ecs::message::MessageReader, prelude::*};
use vmux_command::ReadAppCommands;
use vmux_command::event::SearchEngineSetting;
use vmux_core::{PageOpenRequest, PageOpenTarget};

/// Wires settings: RON load/save with debounce, schema and settings broadcasts, and the
/// settings webview.
pub struct SettingsPlugin;

impl Plugin for SettingsPlugin {
    fn build(&self, app: &mut App) {
        app.world_mut().spawn(crate::PAGE_MANIFEST);
        vmux_core::register_host_spawn(app, "settings");
        app.add_plugins((
            runtime::SettingsRuntimePlugin,
            view::SettingsViewPlugin,
            crate::appearance::AppearancePlugin,
        ))
        .init_resource::<SearchEngineSetting>()
        .init_resource::<vmux_layout::settings::EffectiveStartupUrl>()
        .add_systems(Update, sync_search_engine)
        .add_message::<vmux_core::page::SettingsPageSpawnRequest>()
        .add_systems(Update, respond_settings_spawn.in_set(ReadAppCommands));
    }
}

fn sync_search_engine(
    settings: Option<Res<runtime::AppSettings>>,
    mut search_engine: ResMut<SearchEngineSetting>,
) {
    let Some(settings) = settings else {
        return;
    };
    if search_engine.0 != settings.browser.search_engine {
        search_engine.0 = settings.browser.search_engine;
    }
}

fn respond_settings_spawn(
    mut reader: MessageReader<vmux_core::page::SettingsPageSpawnRequest>,
    mut page_open: MessageWriter<PageOpenRequest>,
) {
    for req in reader.read() {
        page_open.write(PageOpenRequest {
            target: PageOpenTarget::Stack(req.target_stack),
            url: crate::event::SETTINGS_PAGE_URL.to_string(),
            request_id: None,
        });
    }
}
