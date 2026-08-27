mod appearance;
mod runtime;
mod view;

use bevy::{ecs::message::MessageReader, prelude::*};
use vmux_command::ReadAppCommands;
use vmux_core::{PageOpenRequest, PageOpenTarget};

pub use appearance::{ColorSchemeChanged, ResolvedColorScheme, ResolvedScheme, SystemAppearance};
pub use runtime::{
    AcpAgentConfig, AgentSettings, AppSettings, BrowserSettings, ColorScheme, DirSource,
    EXPLORER_DEFAULT_WIDTH, EXPLORER_MAX_WIDTH, EXPLORER_MIN_WIDTH, KeyComboDef, SettingsLoadSet,
    SettingsRuntimePlugin, SettingsSaveRequest, SettingsWriteRequest, ShortcutDef, ShortcutEntry,
    ShortcutSettings, SpaceOverrides, SpaceProject, StartupDir, TerminalSettings, TerminalTheme,
};
pub use view::Settings;
pub use vmux_command::event::SearchEngine;

pub struct SettingsPlugin;

impl Plugin for SettingsPlugin {
    fn build(&self, app: &mut App) {
        app.world_mut().spawn(crate::PAGE_MANIFEST);
        app.add_plugins((
            SettingsRuntimePlugin,
            view::SettingsViewPlugin,
            appearance::AppearancePlugin,
            vmux_layout::LayoutContractPlugin,
        ))
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
