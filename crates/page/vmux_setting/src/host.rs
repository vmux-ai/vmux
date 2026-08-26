pub mod appearance;
pub mod runtime;
pub mod view;

use bevy::{ecs::message::MessageReader, prelude::*};
use vmux_command::ReadAppCommands;
use vmux_command::event::SearchEngineSetting;
use vmux_core::{PageOpenRequest, PageOpenTarget};

pub use appearance::{ColorSchemeChanged, ResolvedColorScheme, ResolvedScheme, SystemAppearance};
pub use runtime::{
    AcpAgentConfig, AgentSettings, AppProviderSettings, AppSettings, AppearanceSettings,
    BrowserSettings, ColorScheme, DirSource, EXPLORER_DEFAULT_WIDTH, EXPLORER_MAX_WIDTH,
    EXPLORER_MIN_WIDTH, EditorSettings, ExplorerSettings, KeyComboDef, LastSelfWriteHash,
    LspServerOverride, LspSettings, SettingsLoadSet, SettingsSaveRequest, SettingsWriteRequest,
    ShortcutDef, ShortcutEntry, ShortcutSettings, SpaceOverrides, SpaceProject, TerminalSettings,
    TerminalTheme, apply_settings_update, load_settings, read_settings_from_disk,
    resolve_startup_dir, resolve_startup_dir_for_tab, resolve_startup_dir_for_tab_with_source,
    resolve_startup_url, resolve_tab_workspace_dir, serialize_settings_to_json, set_at_path,
    validate_tab_workspace_dir,
};
pub use view::Settings;
pub use vmux_command::event::SearchEngine;

pub struct SettingsPlugin;

impl Plugin for SettingsPlugin {
    fn build(&self, app: &mut App) {
        app.world_mut().spawn(crate::PAGE_MANIFEST);
        app.add_plugins((
            runtime::SettingsRuntimePlugin,
            view::SettingsViewPlugin,
            appearance::AppearancePlugin,
            vmux_layout::LayoutContractPlugin,
        ))
        .init_resource::<SearchEngineSetting>()
        .add_systems(Update, sync_search_engine)
        .add_message::<vmux_core::page::SettingsPageSpawnRequest>()
        .add_systems(Update, respond_settings_spawn.in_set(ReadAppCommands));
    }
}

pub const PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "settings",
    title: "Settings",
    title_message_id: Some("settings-title"),
    replaces_command: None,
    keywords: &["preferences", "config"],
    icon: Some(vmux_core::BuiltinIcon::Settings),
    command_bar: true,
};

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
