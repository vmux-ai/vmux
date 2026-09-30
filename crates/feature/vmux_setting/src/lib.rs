#![allow(
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::new_ret_no_self
)]

pub(crate) const FEATURE_MANIFEST: &str = include_str!("feature.ron");

pub mod event;
pub mod schema;
pub mod state;
pub mod themes;

#[cfg(ui)]
pub mod ui;

#[cfg(host)]
mod host;
#[cfg(host)]
pub use host::{
    AcpAgentConfig, AgentSettings, AppSettings, BookmarkFolderSettings, BrowserSettings,
    ColorScheme, ColorSchemeChanged, DirSource, EXPLORER_DEFAULT_WIDTH, EXPLORER_MAX_WIDTH,
    EXPLORER_MIN_WIDTH, KeyComboDef, ResolvedColorScheme, ResolvedScheme, SearchEngine,
    SettingToolPlugin, Settings, SettingsLoadSet, SettingsPlugin, SettingsRuntimePlugin,
    SettingsSaveRequest, SettingsWriteRequest, ShortcutDef, ShortcutEntry, ShortcutSettings,
    SpaceOverrides, SpaceProject, StartupDir, SystemAppearance, TerminalSettings, TerminalTheme,
    UpdateChannel,
};
