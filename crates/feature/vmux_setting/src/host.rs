pub use appearance::{ColorSchemeChanged, ResolvedColorScheme, ResolvedScheme, SystemAppearance};
use bevy::{ecs::message::MessageReader, prelude::*};
pub use runtime::{
    AcpAgentConfig, AgentSettings, AppSettings, AppearanceSettings, BookmarkFolderSettings,
    BrowserSettings, ColorScheme, DirSource, EXPLORER_DEFAULT_WIDTH, EXPLORER_MAX_WIDTH,
    EXPLORER_MIN_WIDTH, KeyComboDef, SettingsLoadSet, SettingsRuntimePlugin, SettingsSaveRequest,
    ShortcutDef, ShortcutEntry, ShortcutSettings, SpaceOverrides, SpaceProject, StartupDir,
    TerminalSettings, TerminalTheme, UpdateChannel,
};
pub use state::Settings;
pub use tool::SettingToolPlugin;
pub use vmux_api::command_bar::SearchEngine;
use vmux_command::ReadCommandRequests;
use vmux_ecs::{PageOpenRequest, PageOpenTarget};

mod agent;
mod appearance;
mod projection;
mod runtime;
mod state;
mod tool;

#[derive(bevy::prelude::Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SearchEngineSetting(pub SearchEngine);

#[vmux_native::page]
pub struct SettingsPlugin;

impl Plugin for SettingsPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(ui)]
        app.add_plugins(crate::ui::SettingsPage::plugin());
        app.add_plugins((
            agent::AgentSettingsPlugin,
            SettingsRuntimePlugin,
            SettingToolPlugin,
            state::StatePlugin,
            projection::ProjectionPlugin,
            appearance::AppearancePlugin,
            vmux_layout::LayoutContractPlugin,
        ))
        .add_message::<vmux_ecs::page::SettingsPageSpawnRequest>()
        .add_systems(Update, respond_settings_spawn.in_set(ReadCommandRequests));
    }
}

fn respond_settings_spawn(
    mut reader: MessageReader<vmux_ecs::page::SettingsPageSpawnRequest>,
    mut page_open: MessageWriter<PageOpenRequest>,
) {
    for req in reader.read() {
        page_open.write(PageOpenRequest {
            target: PageOpenTarget::Stack(req.target_stack),
            url: SettingsPlugin::URL.to_string(),
            request_id: None,
        });
    }
}
