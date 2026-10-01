#![allow(
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::new_ret_no_self
)]

#[cfg(host)]
pub(crate) struct Feature;

#[cfg(host)]
impl vmux_ecs::host::manifest::FeatureManifestSource for Feature {
    const SOURCE: &'static str = include_str!("feature.ron");
}

pub mod event;
pub mod state;
pub mod themes;

#[cfg(host)]
mod schema;

#[cfg(ui)]
pub mod ui;

#[cfg(host)]
mod host;
#[cfg(host)]
pub use host::{
    AcpAgentConfig, AgentSettings, AppSettings, AppearanceSettings, BookmarkFolderSettings,
    BrowserSettings, ColorScheme, ColorSchemeChanged, DirSource, EXPLORER_DEFAULT_WIDTH,
    EXPLORER_MAX_WIDTH, EXPLORER_MIN_WIDTH, KeyComboDef, ResolvedColorScheme, ResolvedScheme,
    SearchEngine, SearchEngineSetting, SettingToolPlugin, Settings, SettingsLoadSet,
    SettingsPlugin, SettingsRuntimePlugin, SettingsSaveRequest, ShortcutDef, ShortcutEntry,
    ShortcutSettings, SpaceOverrides, SpaceProject, StartupDir, SystemAppearance, TerminalSettings,
    TerminalTheme, UpdateChannel,
};
